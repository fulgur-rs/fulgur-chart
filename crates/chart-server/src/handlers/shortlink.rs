use crate::{render::OutputFormat, state::AppState};
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

#[utoipa::path(
    post,
    path = "/chart/create",
    request_body = CreateRequest,
    responses(
        (status = 200, description = "Short link created"),
        (status = 503, description = "Shortlink store is full"),
    ),
    tag = "chart"
)]
pub async fn post_create(
    State(state): State<AppState>,
    Json(req): Json<CreateRequest>,
) -> Response {
    let json = req.chart.to_string();
    // ID はチャート JSON + レンダーパラメータ全体のハッシュ（同スペックで異なるサイズ/フォーマットは別リンク）
    // "_" を番兵として None と Some(0) を区別する。
    // ハッシュは 6 bytes（48bit）: 10000件時の誕生日衝突確率 < 0.001%。
    let id_input = format!(
        "{json}\x00{}\x00{}\x00{}\x00{}",
        req.format.as_str(),
        req.width.map_or_else(|| "_".to_string(), |v| v.to_string()),
        req.height
            .map_or_else(|| "_".to_string(), |v| v.to_string()),
        req.background_color.as_deref().unwrap_or(""),
    );
    let hash = Sha256::digest(id_input.as_bytes());
    let id = hex::encode(&hash[..6]);

    let query = build_query(
        &json,
        req.width,
        req.height,
        req.background_color.as_deref(),
        req.format,
    );
    let url = format!("/chart/s/{id}");

    if state.store.insert(id, query) {
        (StatusCode::OK, Json(json!({"url": url}))).into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({
                "error": "shortlink store is full",
                "code": "STORE_FULL"
            })),
        )
            .into_response()
    }
}

pub async fn get_shortlink(Path(id): Path<String>, State(state): State<AppState>) -> Response {
    match state.store.get(&id) {
        Some(query) => Redirect::temporary(&format!("/chart?{query}")).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": "short link not found",
                "code": "NOT_FOUND"
            })),
        )
            .into_response(),
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
