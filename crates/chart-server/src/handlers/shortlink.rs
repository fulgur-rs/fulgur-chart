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
