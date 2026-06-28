use crate::state::AppState;
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
    #[serde(default = "default_fmt")]
    pub format: String,
}

fn default_fmt() -> String {
    "svg".to_string()
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
    let hash = Sha256::digest(json.as_bytes());
    let id = hex::encode(&hash[..4]);

    let query = build_query(
        &json,
        req.width,
        req.height,
        req.background_color.as_deref(),
        &req.format,
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

fn build_query(json: &str, w: Option<u32>, h: Option<u32>, bkg: Option<&str>, fmt: &str) -> String {
    let encoded = urlencoding::encode(json);
    let mut q = format!("c={encoded}&f={fmt}");
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
