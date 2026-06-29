# chart-server Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** fulgur-chart を HTTP 経由で公開する QuickChart 互換の self-hosted Rust サーバーを実装する。

**Architecture:** `crates/chart-server/` として新 workspace crate を作成し、axum で HTTP サーバーを構築する。`fulgur_chart` crate の `frontend::chartjs::parse` → `guard::validate_spec` → `render::render_chart` / `raster_direct::render_chart_to_png/webp` を呼ぶ。全設定は CLI フラグ + `FULGUR_*` 環境変数で注入する。

**Tech Stack:** Rust, axum 0.8, tokio, tower-http (CORS/compression), tower-governor (rate limit), dashmap, sha2, utoipa, clap

---

## Task 1: Workspace に chart-server crate を追加

**Files:**
- Create: `crates/chart-server/Cargo.toml`
- Create: `crates/chart-server/src/main.rs`
- Modify: `Cargo.toml` (workspace members)

**Step 1: workspace の Cargo.toml に members 追加**

```toml
# Cargo.toml (root)
members = ["crates/fulgur-chart", "crates/fulgur-chart-cli", "crates/chart-server"]
```

**Step 2: crates/chart-server/Cargo.toml を作成**

```toml
[package]
name = "chart-server"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[[bin]]
name = "chart-server"
path = "src/main.rs"

[dependencies]
fulgur-chart = { path = "../fulgur-chart" }
axum = { version = "0.8", features = ["macros"] }
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.6", features = ["cors", "compression-full", "limit"] }
tower-governor = "0.6"
dashmap = "6"
sha2 = "0.10"
hex = "0.4"
serde = { workspace = true }
serde_json = { workspace = true }
clap = { workspace = true }
base64 = "0.22"
utoipa = { version = "5", features = ["axum_extras"] }
utoipa-axum = "0.2"
utoipa-swagger-ui = { version = "9", features = ["axum"] }
```

**Step 3: 最小限の main.rs を作成**

```rust
fn main() {
    println!("chart-server");
}
```

**Step 4: ビルド確認**

```bash
cd .worktrees/feat/chart-server
cargo build -p chart-server 2>&1 | tail -5
```

期待: `Compiling chart-server` → `Finished`

**Step 5: コミット**

```bash
git add Cargo.toml crates/chart-server/
git commit -m "chore: add chart-server crate to workspace"
```

---

## Task 2: Config 構造体（clap + 環境変数）

**Files:**
- Create: `crates/chart-server/src/config.rs`
- Modify: `crates/chart-server/src/main.rs`

**Step 1: config.rs を作成**

```rust
use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "chart-server", about = "fulgur-chart HTTP rendering server")]
pub struct Config {
    #[arg(long, env = "FULGUR_HOST", default_value = "0.0.0.0")]
    pub host: String,

    #[arg(long, env = "FULGUR_PORT", default_value_t = 3000)]
    pub port: u16,

    #[arg(long, env = "FULGUR_MAX_CONCURRENT", default_value_t = num_cpus())]
    pub max_concurrent: usize,

    #[arg(long, env = "FULGUR_MAX_BODY_SIZE", default_value_t = 102_400)]
    pub max_body_size: usize,

    #[arg(long, env = "FULGUR_RENDER_TIMEOUT_MS", default_value_t = 1000)]
    pub render_timeout_ms: u64,

    #[arg(long, env = "FULGUR_SHORTLINK_LIMIT", default_value_t = 10_000)]
    pub shortlink_limit: usize,

    #[arg(long, env = "FULGUR_CORS_ORIGINS", default_value = "*")]
    pub cors_origins: String,

    #[arg(long, env = "FULGUR_RATE_LIMIT", default_value_t = 60)]
    pub rate_limit: u64,

    #[arg(long, env = "FULGUR_LOG_LEVEL", default_value = "info")]
    pub log_level: String,
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
```

**Step 2: main.rs で parse してアドレスを表示**

```rust
mod config;
use clap::Parser;

fn main() {
    let cfg = config::Config::parse();
    println!("listening on {}:{}", cfg.host, cfg.port);
}
```

**Step 3: ビルド確認**

```bash
cargo build -p chart-server 2>&1 | tail -3
cargo run -p chart-server -- --help
```

期待: `--host`, `--port` 等のオプションが表示される

**Step 4: コミット**

```bash
git add crates/chart-server/src/
git commit -m "feat(chart-server): add Config struct with clap + env var support"
```

---

## Task 3: サーバー起動（axum router + tokio）

**Files:**
- Create: `crates/chart-server/src/server.rs`
- Modify: `crates/chart-server/src/main.rs`

**Step 1: server.rs を作成（最小 router）**

```rust
use axum::{Router, routing::get};

pub fn build_router() -> Router {
    Router::new().route("/health", get(health))
}

async fn health() -> &'static str {
    "ok"
}
```

**Step 2: main.rs を非同期に更新**

```rust
mod config;
mod server;
use clap::Parser;

#[tokio::main]
async fn main() {
    let cfg = config::Config::parse();
    let addr = format!("{}:{}", cfg.host, cfg.port);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("chart-server listening on {addr}");
    axum::serve(listener, server::build_router()).await.unwrap();
}
```

**Step 3: 動作確認**

```bash
cargo run -p chart-server &
curl -s http://localhost:3000/health
kill %1
```

期待: `ok`

**Step 4: コミット**

```bash
git add crates/chart-server/src/
git commit -m "feat(chart-server): bootstrap axum server with /health"
```

---

## Task 4: /health と /llms.txt エンドポイント

**Files:**
- Create: `crates/chart-server/src/handlers/mod.rs`
- Create: `crates/chart-server/src/handlers/meta.rs`
- Create: `crates/chart-server/llms.txt`
- Modify: `crates/chart-server/src/server.rs`

**Step 1: llms.txt を作成**

```
# fulgur chart-server

> Render Chart.js v4 specs to SVG, PNG, WebP, or data-URI over HTTP.

## Docs

- [OpenAPI Schema](/openapi.json)
- [Interactive Docs](/docs)

## Quick Start

POST /chart with JSON body:

{"chart":{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}},"format":"svg"}

Returns: SVG string (Content-Type: image/svg+xml)

## Formats

- svg  — inline SVG (default)
- png  — binary PNG
- webp — binary WebP
- data-uri — base64 data URI (use in <img src> or Markdown ![](…))

## Validate Without Rendering

POST /chart/validate — returns {"valid":true} or {"valid":false,"error":"…","code":"…"}

## Short Links

POST /chart/create — returns {"url":"/chart/s/<id>"}
GET  /chart/s/<id> — redirects to /chart?c=…
```

**Step 2: handlers/meta.rs を作成**

```rust
use axum::{Json, response::IntoResponse, http::StatusCode};
use serde_json::json;

pub async fn health() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

pub async fn llms_txt() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/plain; charset=utf-8")],
        include_str!("../llms.txt"),
    )
}
```

**Step 3: handlers/mod.rs を作成**

```rust
pub mod meta;
```

**Step 4: server.rs で routes を登録**

```rust
use axum::{Router, routing::get};
use crate::handlers::meta;

pub fn build_router() -> Router {
    Router::new()
        .route("/health", get(meta::health))
        .route("/llms.txt", get(meta::llms_txt))
}
```

**Step 5: 動作確認**

```bash
cargo run -p chart-server &
curl -s http://localhost:3000/health
curl -s http://localhost:3000/llms.txt | head -3
kill %1
```

**Step 6: コミット**

```bash
git add crates/chart-server/
git commit -m "feat(chart-server): add /health (JSON) and /llms.txt endpoints"
```

---

## Task 5: レンダリングコア（render helper）

**Files:**
- Create: `crates/chart-server/src/render.rs`

**Step 1: render.rs を作成**

```rust
use fulgur_chart::{frontend, guard, ir::ChartSpec, raster_direct, render};

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Svg,
    Png,
    Webp,
    #[serde(rename = "data-uri")]
    DataUri,
}

impl Default for OutputFormat {
    fn default() -> Self { Self::Svg }
}

#[derive(Debug)]
pub enum RenderError {
    Parse(String),
    Validate(String),
    Render(String),
}

impl RenderError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Parse(_) => "PARSE_ERROR",
            Self::Validate(_) => "VALIDATE_ERROR",
            Self::Render(_) => "RENDER_ERROR",
        }
    }
    pub fn message(&self) -> &str {
        match self {
            Self::Parse(m) | Self::Validate(m) | Self::Render(m) => m,
        }
    }
}

pub fn parse_and_validate(json: &str, dsl: &str, strict: bool) -> Result<ChartSpec, RenderError> {
    let spec = match dsl {
        "vegalite" => frontend::vegalite::parse(json, strict),
        _ => frontend::chartjs::parse(json, strict),
    }
    .map_err(RenderError::Parse)?;

    guard::validate_spec(&spec, &guard::InputLimits::default())
        .map_err(RenderError::Validate)?;

    Ok(spec)
}

pub fn render(spec: &ChartSpec, format: OutputFormat, scale: f32) -> Result<Vec<u8>, RenderError> {
    match format {
        OutputFormat::Svg | OutputFormat::DataUri => {
            render::render_chart(spec)
                .map_err(|e| RenderError::Render(e.to_string()))
        }
        OutputFormat::Png => {
            let fb = raster_direct::FrameBuffer::new(
                (spec.width as f32 * scale) as u32,
                (spec.height as f32 * scale) as u32,
            );
            raster_direct::render_chart_to_png(spec, scale, fb)
                .map_err(|e| RenderError::Render(e.to_string()))
        }
        OutputFormat::Webp => {
            let fb = raster_direct::FrameBuffer::new(
                (spec.width as f32 * scale) as u32,
                (spec.height as f32 * scale) as u32,
            );
            raster_direct::render_chart_to_webp(spec, scale, fb)
                .map_err(|e| RenderError::Render(e.to_string()))
        }
    }
}
```

**Step 2: ビルド確認（API シグネチャが正しいか）**

```bash
cargo build -p chart-server 2>&1 | grep -E "error|warning: unused" | head -20
```

期待: error なし（実際の API と齟齬があれば修正する）

**Step 3: コミット**

```bash
git add crates/chart-server/src/render.rs crates/chart-server/src/main.rs
git commit -m "feat(chart-server): add render helper (parse→validate→render)"
```

---

## Task 6: POST /chart + GET /chart ハンドラー

**Files:**
- Create: `crates/chart-server/src/handlers/chart.rs`
- Create: `crates/chart-server/src/response.rs`
- Modify: `crates/chart-server/src/handlers/mod.rs`
- Modify: `crates/chart-server/src/server.rs`

**Step 1: response.rs（ETag・キャッシュヘッダ・エラー）を作成**

```rust
use axum::{
    Json,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use serde_json::json;
use crate::render::{OutputFormat, RenderError};

pub fn etag_value(spec_json: &str) -> String {
    let hash = Sha256::digest(spec_json.as_bytes());
    let short = hex::encode(&hash[..8]);
    format!("\"{short}-v{ver}\"", ver = env!("CARGO_PKG_VERSION"))
}

pub fn cache_headers(etag: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(header::CACHE_CONTROL, "public, max-age=86400, immutable".parse().unwrap());
    h.insert(header::ETAG, etag.parse().unwrap());
    h.insert("X-Fulgur-Version", env!("CARGO_PKG_VERSION").parse().unwrap());
    h.insert(header::VARY, "Accept-Encoding".parse().unwrap());
    h
}

pub fn render_response(bytes: Vec<u8>, format: OutputFormat, etag: &str) -> Response {
    let mut headers = cache_headers(etag);
    match format {
        OutputFormat::Svg => {
            headers.insert(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8".parse().unwrap());
            (StatusCode::OK, headers, bytes).into_response()
        }
        OutputFormat::Png => {
            headers.insert(header::CONTENT_TYPE, "image/png".parse().unwrap());
            (StatusCode::OK, headers, bytes).into_response()
        }
        OutputFormat::Webp => {
            headers.insert(header::CONTENT_TYPE, "image/webp".parse().unwrap());
            (StatusCode::OK, headers, bytes).into_response()
        }
        OutputFormat::DataUri => {
            let b64 = STANDARD.encode(&bytes);
            let uri = format!("data:image/svg+xml;base64,{b64}");
            headers.insert(header::CONTENT_TYPE, "text/plain; charset=utf-8".parse().unwrap());
            (StatusCode::OK, headers, uri).into_response()
        }
    }
}

pub fn error_response(status: StatusCode, err: &RenderError) -> Response {
    (status, Json(json!({
        "error": err.message(),
        "code": err.code(),
    }))).into_response()
}
```

**Step 2: handlers/chart.rs を作成**

```rust
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use crate::{
    config::Config,
    render::{self, OutputFormat, RenderError},
    response::{cache_headers, error_response, etag_value, render_response},
};

#[derive(Deserialize)]
pub struct ChartQuery {
    pub c: Option<String>,
    pub w: Option<u32>,
    pub h: Option<u32>,
    pub bkg: Option<String>,
    #[serde(default)]
    pub f: OutputFormat,
}

#[derive(Deserialize)]
pub struct ChartRequest {
    pub chart: Value,
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(rename = "backgroundColor")]
    pub background_color: Option<String>,
    #[serde(default)]
    pub format: OutputFormat,
    #[serde(default = "default_dsl")]
    pub dsl: String,
}

fn default_dsl() -> String { "chartjs".to_string() }

pub async fn get_chart(
    Query(q): Query<ChartQuery>,
    if_none_match: Option<axum::TypedHeader<axum::headers::IfNoneMatch>>,
) -> Response {
    let Some(c) = q.c else {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "missing required parameter: c",
            "code": "MISSING_PARAM"
        }))).into_response();
    };

    // apply width/height/bkg overrides into the JSON
    let json = apply_overrides(&c, q.w, q.h, q.bkg.as_deref());

    handle_render(&json, q.f, "chartjs", if_none_match).await
}

pub async fn post_chart(
    Json(req): Json<ChartRequest>,
) -> Response {
    let json = req.chart.to_string();
    handle_render(&json, req.format, &req.dsl, None).await
}

async fn handle_render(
    json: &str,
    format: OutputFormat,
    dsl: &str,
    if_none_match: Option<axum::TypedHeader<axum::headers::IfNoneMatch>>,
) -> Response {
    let etag = etag_value(json);

    // 304 check
    if let Some(inm) = if_none_match {
        if inm.0.to_string().contains(&etag) {
            return (StatusCode::NOT_MODIFIED, cache_headers(&etag)).into_response();
        }
    }

    let json_owned = json.to_string();
    let dsl_owned = dsl.to_string();
    let result = tokio::task::spawn_blocking(move || {
        let spec = render::parse_and_validate(&json_owned, &dsl_owned, false)?;
        render::render(&spec, format, 1.0)
    }).await;

    match result {
        Ok(Ok(bytes)) => render_response(bytes, format, &etag),
        Ok(Err(e @ RenderError::Parse(_))) => error_response(StatusCode::BAD_REQUEST, &e),
        Ok(Err(e @ RenderError::Validate(_))) => error_response(StatusCode::BAD_REQUEST, &e),
        Ok(Err(e @ RenderError::Render(_))) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "render task panicked").into_response(),
    }
}

fn apply_overrides(json: &str, w: Option<u32>, h: Option<u32>, bkg: Option<&str>) -> String {
    let Ok(mut v) = serde_json::from_str::<Value>(json) else { return json.to_string() };
    let obj = v.as_object_mut().unwrap();
    if let Some(w) = w { obj.insert("width".into(), w.into()); }
    if let Some(h) = h { obj.insert("height".into(), h.into()); }
    if let Some(bkg) = bkg {
        obj.entry("options").or_insert(Value::Object(Default::default()))
            .as_object_mut().unwrap()
            .insert("backgroundColor".into(), bkg.into());
    }
    v.to_string()
}
```

**Step 3: handlers/mod.rs に追加**

```rust
pub mod chart;
pub mod meta;
```

**Step 4: server.rs に routes 追加**

```rust
use axum::{Router, routing::{get, post}};
use crate::handlers::{chart, meta};

pub fn build_router() -> Router {
    Router::new()
        .route("/health", get(meta::health))
        .route("/llms.txt", get(meta::llms_txt))
        .route("/chart", get(chart::get_chart).post(chart::post_chart))
}
```

**Step 5: ビルド確認**

```bash
cargo build -p chart-server 2>&1 | grep "^error" | head -20
```

**Step 6: 動作確認**

```bash
cargo run -p chart-server &
curl -s -X POST http://localhost:3000/chart \
  -H "Content-Type: application/json" \
  -d '{"chart":{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}}}'  | head -c 200
kill %1
```

期待: `<svg` で始まる SVG レスポンス

**Step 7: コミット**

```bash
git add crates/chart-server/src/
git commit -m "feat(chart-server): implement GET/POST /chart with ETag cache headers"
```

---

## Task 7: POST /chart/validate

**Files:**
- Create: `crates/chart-server/src/handlers/validate.rs`
- Modify: `crates/chart-server/src/handlers/mod.rs`
- Modify: `crates/chart-server/src/server.rs`

**Step 1: handlers/validate.rs を作成**

```rust
use axum::{Json, http::StatusCode, response::{IntoResponse, Response}};
use serde::Deserialize;
use serde_json::{Value, json};
use crate::render;

#[derive(Deserialize)]
pub struct ValidateRequest {
    pub chart: Value,
    #[serde(default = "default_dsl")]
    pub dsl: String,
}

fn default_dsl() -> String { "chartjs".to_string() }

pub async fn post_validate(Json(req): Json<ValidateRequest>) -> Response {
    let json = req.chart.to_string();
    let dsl = req.dsl.clone();
    let result = tokio::task::spawn_blocking(move || {
        render::parse_and_validate(&json, &dsl, false)
    }).await;

    match result {
        Ok(Ok(_)) => (StatusCode::OK, Json(json!({"valid": true}))).into_response(),
        Ok(Err(e)) => (StatusCode::BAD_REQUEST, Json(json!({
            "valid": false,
            "error": e.message(),
            "code": e.code(),
        }))).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "task panicked").into_response(),
    }
}
```

**Step 2: mod.rs・server.rs に追加**

mod.rs:
```rust
pub mod chart;
pub mod meta;
pub mod validate;
```

server.rs に追加:
```rust
.route("/chart/validate", post(validate::post_validate))
```

**Step 3: 動作確認**

```bash
cargo run -p chart-server &
# valid
curl -s -X POST http://localhost:3000/chart/validate \
  -H "Content-Type: application/json" \
  -d '{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}'
# invalid
curl -s -X POST http://localhost:3000/chart/validate \
  -H "Content-Type: application/json" \
  -d '{"chart":{"type":"unknown_type"}}'
kill %1
```

期待1: `{"valid":true}`  
期待2: `{"valid":false,"error":"...","code":"PARSE_ERROR"}`

**Step 4: コミット**

```bash
git add crates/chart-server/src/handlers/validate.rs crates/chart-server/src/handlers/mod.rs crates/chart-server/src/server.rs
git commit -m "feat(chart-server): implement POST /chart/validate"
```

---

## Task 8: 短縮URL（POST /chart/create + GET /chart/s/{id}）

**Files:**
- Create: `crates/chart-server/src/store.rs`
- Create: `crates/chart-server/src/handlers/shortlink.rs`
- Modify: `crates/chart-server/src/handlers/mod.rs`
- Modify: `crates/chart-server/src/server.rs`
- Modify: `crates/chart-server/src/main.rs`

**Step 1: store.rs を作成**

```rust
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct ShortlinkStore {
    map: Arc<DashMap<String, String>>,
    limit: usize,
}

impl ShortlinkStore {
    pub fn new(limit: usize) -> Self {
        Self { map: Arc::new(DashMap::new()), limit }
    }

    pub fn insert(&self, id: String, json: String) -> bool {
        if self.map.len() >= self.limit {
            return false;
        }
        self.map.insert(id, json);
        true
    }

    pub fn get(&self, id: &str) -> Option<String> {
        self.map.get(id).map(|v| v.clone())
    }
}
```

**Step 2: handlers/shortlink.rs を作成**

```rust
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use crate::store::ShortlinkStore;

#[derive(Deserialize)]
pub struct CreateRequest {
    pub chart: Value,
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(rename = "backgroundColor")]
    pub background_color: Option<String>,
    #[serde(default = "default_fmt")]
    pub format: String,
}

fn default_fmt() -> String { "svg".to_string() }

pub async fn post_create(
    State(store): State<ShortlinkStore>,
    Json(req): Json<CreateRequest>,
) -> Response {
    let json = req.chart.to_string();
    let hash = Sha256::digest(json.as_bytes());
    let id = hex::encode(&hash[..4]);

    let query = build_query(&json, req.width, req.height, req.background_color.as_deref(), &req.format);
    let url = format!("/chart/s/{id}");

    if store.insert(id, query) {
        (StatusCode::OK, Json(json!({"url": url}))).into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, Json(json!({
            "error": "shortlink store is full",
            "code": "STORE_FULL"
        }))).into_response()
    }
}

pub async fn get_shortlink(
    Path(id): Path<String>,
    State(store): State<ShortlinkStore>,
) -> Response {
    match store.get(&id) {
        Some(query) => Redirect::temporary(&format!("/chart?{query}")).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({
            "error": "short link not found",
            "code": "NOT_FOUND"
        }))).into_response(),
    }
}

fn build_query(json: &str, w: Option<u32>, h: Option<u32>, bkg: Option<&str>, fmt: &str) -> String {
    let encoded = urlencoding::encode(json);
    let mut q = format!("c={encoded}&f={fmt}");
    if let Some(w) = w { q.push_str(&format!("&w={w}")); }
    if let Some(h) = h { q.push_str(&format!("&h={h}")); }
    if let Some(bkg) = bkg { q.push_str(&format!("&bkg={}", urlencoding::encode(bkg))); }
    q
}
```

**Step 3: Cargo.toml に urlencoding を追加**

```toml
urlencoding = "2"
```

**Step 4: server.rs を AppState 対応に更新**

```rust
use axum::{Router, routing::{get, post}};
use crate::{
    handlers::{chart, meta, shortlink, validate},
    store::ShortlinkStore,
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
```

**Step 5: main.rs を更新**

```rust
let store = store::ShortlinkStore::new(cfg.shortlink_limit);
axum::serve(listener, server::build_router(store)).await.unwrap();
```

**Step 6: 動作確認**

```bash
cargo run -p chart-server &
# 短縮URL作成
RESP=$(curl -s -X POST http://localhost:3000/chart/create \
  -H "Content-Type: application/json" \
  -d '{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}')
echo $RESP
ID=$(echo $RESP | grep -o '"url":"/chart/s/[^"]*"' | cut -d'"' -f4 | cut -d'/' -f4)
# リダイレクト確認
curl -v http://localhost:3000/chart/s/$ID 2>&1 | grep "Location"
kill %1
```

**Step 7: コミット**

```bash
git add crates/chart-server/src/
git commit -m "feat(chart-server): implement POST /chart/create and GET /chart/s/{id}"
```

---

## Task 9: ミドルウェア（圧縮・CORS・body制限・同時実行・タイムアウト・レート制限）

**Files:**
- Modify: `crates/chart-server/src/server.rs`
- Modify: `crates/chart-server/src/handlers/chart.rs`
- Modify: `crates/chart-server/src/config.rs`

**Step 1: server.rs に tower-http ミドルウェアを追加**

```rust
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    limit::RequestBodyLimitLayer,
};
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
use std::time::Duration;

pub fn build_router(store: ShortlinkStore, cfg: &crate::config::Config) -> Router {
    let cors = if cfg.cors_origins == "*" {
        CorsLayer::new().allow_origin(Any)
    } else {
        let origins: Vec<_> = cfg.cors_origins.split(',')
            .filter_map(|o| o.trim().parse().ok())
            .collect();
        CorsLayer::new().allow_origin(origins)
    };

    let governor_conf = GovernorConfigBuilder::default()
        .per_second(cfg.rate_limit)
        .burst_size(cfg.rate_limit as u32)
        .finish()
        .unwrap();

    Router::new()
        /* ... routes ... */
        .layer(CompressionLayer::new())
        .layer(cors)
        .layer(RequestBodyLimitLayer::new(cfg.max_body_size))
        .layer(GovernorLayer { config: Arc::new(governor_conf) })
}
```

**Step 2: chart.rs のレンダリングに Semaphore + timeout を追加**

```rust
use tokio::{sync::Semaphore, time};
use std::sync::Arc;

// AppState に追加
#[derive(Clone)]
pub struct AppState {
    pub store: ShortlinkStore,
    pub semaphore: Arc<Semaphore>,
    pub timeout_ms: u64,
}

// handle_render 内で使用
let permit = match time::timeout(
    Duration::from_millis(100),
    state.semaphore.acquire()
).await {
    Ok(Ok(p)) => p,
    _ => return (StatusCode::SERVICE_UNAVAILABLE, "server busy").into_response(),
};

let result = time::timeout(
    Duration::from_millis(state.timeout_ms),
    tokio::task::spawn_blocking(move || { /* render */ })
).await;

drop(permit);

match result {
    Err(_) => (StatusCode::GATEWAY_TIMEOUT, Json(json!({"code":"TIMEOUT","error":"render timed out"}))).into_response(),
    /* ... */
}
```

**Step 3: ビルド・動作確認**

```bash
cargo build -p chart-server 2>&1 | grep "^error" | head -10
cargo run -p chart-server &
curl -v -X POST http://localhost:3000/chart \
  -H "Content-Type: application/json" \
  -d '{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}' \
  2>&1 | grep -E "< (Content-Encoding|Cache-Control|ETag|X-Fulgur)"
kill %1
```

期待: レスポンスヘッダに `Cache-Control`, `ETag`, `X-Fulgur-Version` が含まれる

**Step 4: コミット**

```bash
git add crates/chart-server/src/
git commit -m "feat(chart-server): add compression, CORS, rate limit, concurrency, timeout middleware"
```

---

## Task 10: GET /openapi.json + GET /docs

**Files:**
- Modify: `crates/chart-server/src/server.rs`（utoipa router 統合）
- Modify: `crates/chart-server/src/handlers/chart.rs`（utoipa アノテーション追加）

**Step 1: utoipa の ApiDoc を定義**

server.rs に追加:
```rust
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use utoipa_axum::router::OpenApiRouter;

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::chart::post_chart,
        crate::handlers::validate::post_validate,
    ),
    components(schemas(
        crate::handlers::chart::ChartRequest,
        crate::handlers::validate::ValidateRequest,
    ))
)]
struct ApiDoc;

// router に追加
let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
    /* ... routes ... */
    .split_for_parts();

let router = router
    .route("/openapi.json", get(|| async { Json(api) }))
    .merge(SwaggerUi::new("/docs").url("/openapi.json", ApiDoc::openapi()));
```

**Step 2: ハンドラーに #[utoipa::path] を追加**

```rust
#[utoipa::path(
    post,
    path = "/chart",
    request_body = ChartRequest,
    responses(
        (status = 200, description = "SVG/PNG/WebP bytes"),
        (status = 400, description = "Parse or validation error"),
    )
)]
pub async fn post_chart(...) { ... }
```

**Step 3: 動作確認**

```bash
cargo run -p chart-server &
curl -s http://localhost:3000/openapi.json | python3 -m json.tool | head -20
curl -I http://localhost:3000/docs
kill %1
```

**Step 4: コミット**

```bash
git add crates/chart-server/src/
git commit -m "feat(chart-server): add /openapi.json and /docs (Swagger UI)"
```

---

## Task 11: Docker イメージ + CI

**Files:**
- Create: `crates/chart-server/Dockerfile`
- Create: `.github/workflows/chart-server-docker.yml`

**Step 1: Dockerfile を作成**

```dockerfile
FROM rust:1.89-slim AS builder
WORKDIR /build
COPY . .
RUN cargo build --release -p chart-server

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /build/target/release/chart-server /usr/local/bin/chart-server
EXPOSE 3000
ENTRYPOINT ["chart-server"]
```

**Step 2: GitHub Actions ワークフローを作成**

```yaml
name: chart-server Docker

on:
  push:
    tags: ["chart-server-v*"]
  workflow_dispatch:

jobs:
  docker:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v4
      - uses: docker/login-action@v3
        with:
          registry: ghcr.io
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}
      - uses: docker/build-push-action@v6
        with:
          context: .
          file: crates/chart-server/Dockerfile
          push: true
          tags: ghcr.io/fulgur-rs/chart-server:latest
```

**Step 3: ローカルでビルド確認**

```bash
docker build -f crates/chart-server/Dockerfile -t chart-server:local . 2>&1 | tail -5
docker run --rm -p 3000:3000 chart-server:local &
curl -s http://localhost:3000/health
docker stop $(docker ps -q --filter ancestor=chart-server:local)
```

**Step 4: コミット**

```bash
git add crates/chart-server/Dockerfile .github/workflows/chart-server-docker.yml
git commit -m "feat(chart-server): add Dockerfile and ghcr.io CI workflow"
```

---

## Task 12: Railway + Cloudflare Workers Containers デプロイ設定

**Files:**
- Create: `crates/chart-server/railway.toml`
- Create: `crates/chart-server/wrangler.toml`

**Step 1: railway.toml を作成**

```toml
[build]
builder = "DOCKERFILE"
dockerfilePath = "crates/chart-server/Dockerfile"

[deploy]
startCommand = "chart-server"
healthcheckPath = "/health"
healthcheckTimeout = 10
restartPolicyType = "ON_FAILURE"
```

**Step 2: wrangler.toml を作成（Cloudflare Workers Containers）**

```toml
name = "chart-server"
compatibility_date = "2025-01-01"

[containers]
image = "ghcr.io/fulgur-rs/chart-server:latest"
max_instances = 10

[[containers.ports]]
port = 3000
```

**Step 3: コミット**

```bash
git add crates/chart-server/railway.toml crates/chart-server/wrangler.toml
git commit -m "feat(chart-server): add Railway and Cloudflare Workers Containers deploy configs"
```

---

## Task 13: MCP エンドポイント（GET /mcp）

> **Note:** Streamable HTTP (MCP 2025-03-26) の実装。`rmcp` クレートを使用する。

**Files:**
- Modify: `crates/chart-server/Cargo.toml`
- Create: `crates/chart-server/src/handlers/mcp.rs`
- Modify: `crates/chart-server/src/server.rs`

**Step 1: rmcp を依存に追加**

```toml
rmcp = { version = "0.2", features = ["server", "transport-streamable-http"] }
```

**Step 2: handlers/mcp.rs で generate_chart ツールを実装**

`rmcp` の `ServerHandler` トレイトを実装し、`generate_chart` ツールを登録する。入力は `ChartRequest` と同じ JSON スキーマ。出力は SVG テキストまたは Base64 PNG を MCP `content` として返す。

詳細は `rmcp` ドキュメント参照: https://docs.rs/rmcp

**Step 3: server.rs に /mcp route を追加**

```rust
.route("/mcp", post(mcp::handle_mcp))
```

**Step 4: 動作確認**

MCP Inspector または curl で initialize → tools/list → tools/call を確認。

**Step 5: コミット**

```bash
git add crates/chart-server/src/handlers/mcp.rs crates/chart-server/src/server.rs crates/chart-server/Cargo.toml
git commit -m "feat(chart-server): add MCP Streamable HTTP endpoint (/mcp)"
```

---

## 完了チェックリスト

```bash
# 全エンドポイント確認
curl http://localhost:3000/health
curl http://localhost:3000/llms.txt
curl http://localhost:3000/openapi.json | python3 -m json.tool | head -5
curl http://localhost:3000/docs -I
curl -X POST http://localhost:3000/chart -H "Content-Type: application/json" \
  -d '{"chart":{"type":"bar","data":{"labels":["A","B"],"datasets":[{"data":[1,2]}]}}}'  | head -c 50
curl -X POST http://localhost:3000/chart/validate -H "Content-Type: application/json" \
  -d '{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}'
# ETag ヘッダ確認
curl -I -X POST http://localhost:3000/chart -H "Content-Type: application/json" \
  -d '{"chart":{"type":"bar","data":{"labels":["A"],"datasets":[{"data":[1]}]}}}' \
  | grep -E "ETag|Cache-Control|X-Fulgur"
```
