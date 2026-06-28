use crate::{
    render::{self, OutputFormat, RenderError},
    response::{cache_headers, error_response, etag_value, render_response},
    state::AppState,
};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct ChartQuery {
    /// Chart.js JSON spec (URL-encoded)
    pub c: Option<String>,
    /// Width in pixels
    pub w: Option<u32>,
    /// Height in pixels
    pub h: Option<u32>,
    /// Background colour (e.g. `white`)
    pub bkg: Option<String>,
    /// Output format: `svg`, `png`, `webp`, `data-uri`
    #[serde(default)]
    pub f: OutputFormat,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ChartRequest {
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
    /// DSL frontend (default: `chartjs`)
    #[serde(default = "default_dsl")]
    pub dsl: String,
}

fn default_dsl() -> String {
    "chartjs".to_string()
}

#[utoipa::path(
    get,
    path = "/chart",
    params(ChartQuery),
    responses(
        (status = 200, description = "Chart rendered successfully"),
        (status = 304, description = "Not Modified (ETag match)"),
        (status = 400, description = "Invalid chart spec or missing parameter"),
        (status = 503, description = "Server busy"),
        (status = 504, description = "Render timeout"),
    ),
    tag = "chart"
)]
pub async fn get_chart(
    State(state): State<AppState>,
    Query(q): Query<ChartQuery>,
    headers: HeaderMap,
) -> Response {
    let Some(c) = q.c else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "missing required parameter: c",
                "code": "MISSING_PARAM"
            })),
        )
            .into_response();
    };
    let json = apply_overrides(&c, q.w, q.h, q.bkg.as_deref());
    handle_render(json, q.f, "chartjs".to_string(), headers, state).await
}

#[utoipa::path(
    post,
    path = "/chart",
    request_body = ChartRequest,
    responses(
        (status = 200, description = "Chart rendered successfully"),
        (status = 304, description = "Not Modified (ETag match)"),
        (status = 400, description = "Invalid chart spec"),
        (status = 503, description = "Server busy"),
        (status = 504, description = "Render timeout"),
    ),
    tag = "chart"
)]
pub async fn post_chart(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ChartRequest>,
) -> Response {
    let json = apply_overrides_value(
        req.chart,
        req.width,
        req.height,
        req.background_color.as_deref(),
    )
    .to_string();
    handle_render(json, req.format, req.dsl, headers, state).await
}

async fn handle_render(
    json: String,
    format: OutputFormat,
    dsl: String,
    headers: HeaderMap,
    state: AppState,
) -> Response {
    let etag = etag_value(&json, format);

    // 304 check (RFC 7232 compliant)
    if let Some(inm) = headers.get(axum::http::header::IF_NONE_MATCH)
        && let Ok(inm_str) = inm.to_str()
    {
        let etag_bare = etag.trim_matches('"');
        let matches = inm_str.trim() == "*"
            || inm_str
                .split(',')
                .map(|s| {
                    s.trim()
                        .trim_matches('"')
                        .trim_start_matches("W/")
                        .trim_matches('"')
                })
                .any(|candidate| candidate == etag_bare);
        if matches {
            return (StatusCode::NOT_MODIFIED, cache_headers(&etag)).into_response();
        }
    }

    // Semaphore 取得（超過時 503）
    let permit = match state.semaphore.try_acquire_owned() {
        Ok(p) => p,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                [("Retry-After", "1")],
                Json(json!({"error": "server busy", "code": "BUSY"})),
            )
                .into_response();
        }
    };

    // タイムアウト付きレンダリング
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(state.render_timeout_ms),
        tokio::task::spawn_blocking(move || {
            let _permit = permit; // クロージャ完了まで permit を保持して Semaphore を正しく解放
            let spec = render::parse_and_validate(&json, &dsl, false)?;
            render::render(&spec, format, 1.0)
        }),
    )
    .await;

    match result {
        Err(_timeout) => (
            StatusCode::GATEWAY_TIMEOUT,
            Json(json!({"error": "render timeout", "code": "TIMEOUT"})),
        )
            .into_response(),
        Ok(Ok(Ok(bytes))) => render_response(bytes, format, &etag),
        Ok(Ok(Err(e @ RenderError::Parse(_)))) => error_response(StatusCode::BAD_REQUEST, &e),
        Ok(Ok(Err(e @ RenderError::Validate(_)))) => error_response(StatusCode::BAD_REQUEST, &e),
        Ok(Ok(Err(e @ RenderError::Render(_)))) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, &e)
        }
        Ok(Err(_join_err)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "render task panicked").into_response()
        }
    }
}

pub(crate) fn apply_overrides_value(
    mut v: Value,
    w: Option<u32>,
    h: Option<u32>,
    bkg: Option<&str>,
) -> Value {
    if let Some(obj) = v.as_object_mut() {
        if let Some(w) = w {
            obj.insert("width".into(), w.into());
        }
        if let Some(h) = h {
            obj.insert("height".into(), h.into());
        }
        if let Some(bkg) = bkg {
            let options = obj
                .entry("options")
                .or_insert_with(|| Value::Object(Default::default()));
            if !options.is_object() {
                *options = Value::Object(Default::default());
            }
            if let Some(opts_obj) = options.as_object_mut() {
                let theme = opts_obj
                    .entry("theme")
                    .or_insert_with(|| Value::Object(Default::default()));
                if !theme.is_object() {
                    *theme = Value::Object(Default::default());
                }
                if let Some(theme_obj) = theme.as_object_mut() {
                    theme_obj.insert("backgroundColor".into(), bkg.into());
                }
            }
        }
    }
    v
}

fn apply_overrides(json: &str, w: Option<u32>, h: Option<u32>, bkg: Option<&str>) -> String {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return json.to_string();
    };
    apply_overrides_value(v, w, h, bkg).to_string()
}
