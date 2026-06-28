use crate::{render, state::AppState};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ValidateRequest {
    /// Chart.js v4 spec as JSON object
    pub chart: Value,
    /// DSL frontend (default: `chartjs`)
    #[serde(default = "default_dsl")]
    pub dsl: String,
}

fn default_dsl() -> String {
    "chartjs".to_string()
}

#[utoipa::path(
    post,
    path = "/chart/validate",
    request_body = ValidateRequest,
    responses(
        (status = 200, description = "Spec is valid"),
        (status = 400, description = "Spec is invalid"),
        (status = 503, description = "Server busy"),
        (status = 504, description = "Timeout"),
        (status = 500, description = "Internal error"),
    ),
    tag = "chart"
)]
pub async fn post_validate(
    State(state): State<AppState>,
    Json(req): Json<ValidateRequest>,
) -> Response {
    let permit = match state.semaphore.try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"valid": false, "error": "server busy", "code": "BUSY"})),
            )
                .into_response();
        }
    };

    let json = req.chart.to_string();
    let dsl = req.dsl.clone();
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(state.render_timeout_ms),
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            render::parse_and_validate(&json, &dsl, false)
        }),
    )
    .await;

    match result {
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            Json(json!({"valid": false, "error": "validate timeout", "code": "TIMEOUT"})),
        )
            .into_response(),
        Ok(Ok(Ok(_))) => (StatusCode::OK, Json(json!({"valid": true}))).into_response(),
        Ok(Ok(Err(e))) => (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "valid": false,
                "error": e.message(),
                "code": e.code(),
            })),
        )
            .into_response(),
        Ok(Err(_)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "valid": false,
                "error": "internal error",
                "code": "INTERNAL_ERROR"
            })),
        )
            .into_response(),
    }
}
