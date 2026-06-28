use crate::{
    render::{self, OutputFormat, RenderError},
    response::{cache_headers, error_response, etag_value, render_response},
};
use axum::{
    Json,
    extract::Query,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::Value;

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
#[allow(dead_code)]
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

fn default_dsl() -> String {
    "chartjs".to_string()
}

pub async fn get_chart(Query(q): Query<ChartQuery>, headers: HeaderMap) -> Response {
    let Some(c) = q.c else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "missing required parameter: c",
                "code": "MISSING_PARAM"
            })),
        )
            .into_response();
    };
    let json = apply_overrides(&c, q.w, q.h, q.bkg.as_deref());
    handle_render(json, q.f, "chartjs".to_string(), headers).await
}

pub async fn post_chart(headers: HeaderMap, Json(req): Json<ChartRequest>) -> Response {
    let json = req.chart.to_string();
    handle_render(json, req.format, req.dsl, headers).await
}

async fn handle_render(
    json: String,
    format: OutputFormat,
    dsl: String,
    headers: HeaderMap,
) -> Response {
    let etag = etag_value(&json);

    // 304 check (RFC 7232 compliant)
    if let Some(inm) = headers.get(axum::http::header::IF_NONE_MATCH) {
        if let Ok(inm_str) = inm.to_str() {
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
    }

    let result = tokio::task::spawn_blocking(move || {
        let spec = render::parse_and_validate(&json, &dsl, false)?;
        render::render(&spec, format, 1.0)
    })
    .await;

    match result {
        Ok(Ok(bytes)) => render_response(bytes, format, &etag),
        Ok(Err(e @ RenderError::Parse(_))) => error_response(StatusCode::BAD_REQUEST, &e),
        Ok(Err(e @ RenderError::Validate(_))) => error_response(StatusCode::BAD_REQUEST, &e),
        Ok(Err(e @ RenderError::Render(_))) => {
            error_response(StatusCode::INTERNAL_SERVER_ERROR, &e)
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "render task panicked").into_response(),
    }
}

fn apply_overrides(json: &str, w: Option<u32>, h: Option<u32>, bkg: Option<&str>) -> String {
    let Ok(mut v) = serde_json::from_str::<Value>(json) else {
        return json.to_string();
    };
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
                .or_insert(Value::Object(Default::default()));
            if let Some(opts_obj) = options.as_object_mut() {
                opts_obj.insert("backgroundColor".into(), bkg.into());
            }
        }
    }
    v.to_string()
}
