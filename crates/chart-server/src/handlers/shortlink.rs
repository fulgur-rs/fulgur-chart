use crate::store::ShortlinkStore;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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

fn default_fmt() -> String {
    "svg".to_string()
}

pub async fn post_create(
    State(store): State<ShortlinkStore>,
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

    if store.insert(id, query) {
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

pub async fn get_shortlink(
    Path(id): Path<String>,
    State(store): State<ShortlinkStore>,
) -> Response {
    match store.get(&id) {
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
