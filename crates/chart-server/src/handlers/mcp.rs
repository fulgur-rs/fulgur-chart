use crate::{
    render::{self, OutputFormat},
    state::AppState,
};
use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use serde_json::{Map, Value, json};

// JSON-RPC 2.0 レスポンス
#[derive(Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    // id は null の場合も必ず出力する（JSON-RPC 2.0: error 時は "id": null が必須）。
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i64, message: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }
}

pub async fn mcp_handler(
    State(state): State<AppState>,
    // Bytes で受け取り自前でパースする。axum の Json<V> エクストラクタは malformed JSON を
    // 422 で返してしまい JSON-RPC の -32700 Parse error を返せないため。
    body: Bytes,
) -> Response {
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(JsonRpcResponse::error(None, -32700, "Parse error".into())),
            )
                .into_response();
        }
    };
    match raw {
        Value::Array(batch) => {
            // MCP 2025-03-26 Streamable HTTP: 全件 notification の batch → 202 Accepted。
            // request を含む batch は未サポート（experimental）。
            // 各要素を単体リクエスト側と同様に検証してから notification と判定する。
            let all_notifications = !batch.is_empty()
                && batch.iter().all(|item| {
                    item.as_object().is_some_and(|obj| {
                        obj.get("jsonrpc").and_then(|v| v.as_str()) == Some("2.0")
                            && obj.get("method").and_then(|v| v.as_str()).is_some()
                            && !obj.contains_key("id")
                    })
                });
            if all_notifications {
                StatusCode::ACCEPTED.into_response()
            } else {
                (
                    StatusCode::BAD_REQUEST,
                    Json(JsonRpcResponse::error(
                        None,
                        -32600,
                        "Batch requests are not supported".into(),
                    )),
                )
                    .into_response()
            }
        }
        Value::Object(obj) => handle_single(obj, state).await,
        _ => (
            StatusCode::BAD_REQUEST,
            Json(JsonRpcResponse::error(None, -32700, "Parse error".into())),
        )
            .into_response(),
    }
}

/// valid な形式（string / 整数）の id を取り出す。invalid な型は None を返す。
fn extract_valid_id(raw: &Map<String, Value>) -> Option<Value> {
    match raw.get("id") {
        Some(v @ Value::String(_)) => Some(v.clone()),
        Some(Value::Number(n)) if n.is_i64() || n.is_u64() => Some(Value::Number(n.clone())),
        _ => None,
    }
}

async fn handle_single(raw: Map<String, Value>, state: AppState) -> Response {
    // valid な id を先に取り出す。early validation 失敗時のレスポンスにも echo back するため。
    // JSON-RPC 2.0: クライアントはレスポンスの id でリクエストを照合する。
    let early_id = extract_valid_id(&raw);

    // jsonrpc と method を先に検証してから notification / request を判定する。
    // malformed な object（{"foo":"bar"} 等）が notification として 202 になるのを防ぐ。
    let jsonrpc = raw.get("jsonrpc").and_then(|v| v.as_str()).unwrap_or("");
    if jsonrpc != "2.0" {
        return (
            StatusCode::BAD_REQUEST,
            Json(JsonRpcResponse::error(
                early_id,
                -32600,
                "Invalid Request".into(),
            )),
        )
            .into_response();
    }

    if raw.get("method").and_then(|v| v.as_str()).is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(JsonRpcResponse::error(
                early_id,
                -32600,
                "Invalid Request".into(),
            )),
        )
            .into_response();
    }

    // notification: jsonrpc と method が有効で id キーが存在しない。
    // MCP 2025-03-26 Streamable HTTP: notification には 202 Accepted を返す。
    if !raw.contains_key("id") {
        return StatusCode::ACCEPTED.into_response();
    }

    // MCP 2025-03-26: request id は string または整数のみ（小数/null/object/array/bool は不正）。
    let id = match extract_valid_id(&raw) {
        Some(v) => Some(v),
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(JsonRpcResponse::error(
                    None,
                    -32600,
                    "Invalid Request: id must be string or integer".into(),
                )),
            )
                .into_response();
        }
    };

    let method = raw.get("method").and_then(|v| v.as_str()).unwrap();
    let params = raw.get("params").cloned();

    let result = match method {
        "initialize" => handle_initialize(),
        "tools/list" => handle_tools_list(),
        "tools/call" => handle_tools_call(params, state).await,
        _ => Err((-32601, "Method not found".to_string())),
    };

    match result {
        Ok(v) => (StatusCode::OK, Json(JsonRpcResponse::success(id, v))).into_response(),
        Err((code, msg)) => {
            (StatusCode::OK, Json(JsonRpcResponse::error(id, code, msg))).into_response()
        }
    }
}

fn handle_initialize() -> Result<Value, (i64, String)> {
    Ok(json!({
        "protocolVersion": "2025-03-26",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "fulgur-chart-server",
            "version": env!("CARGO_PKG_VERSION")
        }
    }))
}

fn handle_tools_list() -> Result<Value, (i64, String)> {
    Ok(json!({
        "tools": [
            {
                "name": "generate_chart",
                "description": "Render a Chart.js v4 spec to SVG or PNG. Returns SVG string (format=svg) or base64 data URI (format=png/webp/data-uri).",
                "inputSchema": {
                    "type": "object",
                    "required": ["chart"],
                    "properties": {
                        "chart": {
                            "type": "object",
                            "description": "Chart.js v4 spec (type, data, options)"
                        },
                        "format": {
                            "type": "string",
                            "enum": ["svg", "png", "webp", "data-uri"],
                            "default": "png",
                            "description": "Output format"
                        },
                        "width": { "type": "integer", "description": "Width in px" },
                        "height": { "type": "integer", "description": "Height in px" }
                    }
                }
            }
        ]
    }))
}

async fn handle_tools_call(params: Option<Value>, state: AppState) -> Result<Value, (i64, String)> {
    let params = params.ok_or((-32602, "Missing params".to_string()))?;
    let tool_name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or((-32602, "Missing tool name".to_string()))?;

    if tool_name != "generate_chart" {
        return Err((-32602, format!("Unknown tool: {tool_name}")));
    }

    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let chart_spec = args
        .get("chart")
        .ok_or((-32602, "Missing required argument: chart".to_string()))?
        .clone();

    // フィールドが存在するが型が違う場合は黙って無視せず -32602 を返す。
    let format_str = match args.get("format") {
        None => "png",
        Some(v) => v
            .as_str()
            .ok_or((-32602, "format must be a string".to_string()))?,
    };
    let format: OutputFormat = match format_str {
        "svg" => OutputFormat::Svg,
        "png" => OutputFormat::Png,
        "webp" => OutputFormat::Webp,
        "data-uri" => OutputFormat::DataUri,
        other => return Err((-32602, format!("Unsupported format: {other}"))),
    };

    let width = match args.get("width") {
        None => None,
        Some(v) => {
            let n = v
                .as_u64()
                .ok_or((-32602, "width must be a non-negative integer".to_string()))?;
            Some(u32::try_from(n).map_err(|_| (-32602, "width out of u32 range".to_string()))?)
        }
    };
    let height = match args.get("height") {
        None => None,
        Some(v) => {
            let n = v
                .as_u64()
                .ok_or((-32602, "height must be a non-negative integer".to_string()))?;
            Some(u32::try_from(n).map_err(|_| (-32602, "height out of u32 range".to_string()))?)
        }
    };

    let json_str = super::chart::apply_overrides_value(chart_spec, width, height, None).to_string();

    let permit = state
        .semaphore
        .try_acquire_owned()
        .map_err(|_| (-32000, "Server busy".to_string()))?;

    let compression = state.png_compression;
    let webp = state.webp;
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(state.render_timeout_ms),
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let spec = render::parse_and_validate(&json_str, "chartjs", false)?;
            // 圧縮・WebP ポリシーはサーバ起動時設定を用いる（MCP も per-request 指定なし）。
            render::render(&spec, format, 1.0, compression, webp)
        }),
    )
    .await;

    match result {
        Err(_) => Err((-32000, "Render timeout".to_string())),
        Ok(Err(_)) => Err((-32000, "Render task panicked".to_string())),
        // エラー種別ごとに JSON-RPC エラーコードを使い分ける
        Ok(Ok(Err(e))) => {
            let code = match &e {
                // -32700 は JSON-RPC プロトコルレベルのパースエラー専用。
                // chart spec のパース失敗はツール引数の問題なので -32602。
                render::RenderError::Parse(_) => -32602, // Invalid params
                render::RenderError::Validate(_) => -32602, // Invalid params
                // WebP 無効など、要求フォーマットが受け付けられない場合もツール引数の問題。
                render::RenderError::Unsupported(_) => -32602, // Invalid params
                render::RenderError::Render(_) => -32603,      // Internal error
            };
            Err((code, e.message().to_string()))
        }
        Ok(Ok(Ok(bytes))) => {
            use base64::{Engine, engine::general_purpose::STANDARD};
            let content = match format {
                OutputFormat::DataUri => {
                    // SVG を base64 エンコードして data URI に変換
                    format!("data:image/svg+xml;base64,{}", STANDARD.encode(&bytes))
                }
                OutputFormat::Svg => String::from_utf8_lossy(&bytes).into_owned(),
                OutputFormat::Png | OutputFormat::Webp => {
                    let mime = if format == OutputFormat::Webp {
                        "image/webp"
                    } else {
                        "image/png"
                    };
                    format!("data:{mime};base64,{}", STANDARD.encode(&bytes))
                }
            };
            Ok(json!({
                "content": [
                    {
                        "type": "text",
                        "text": content
                    }
                ]
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, routing::post};
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use tokio::sync::Semaphore;
    use tower::ServiceExt;

    use crate::{file_store::FileShortlinkStore, state::AppState};

    async fn test_app() -> axum::Router {
        // tempdir-backed の durable store。TempDir はリークしてテスト実行中 dir を保持する。
        let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
        let store = FileShortlinkStore::new(dir.path(), 512 * 1024)
            .await
            .unwrap();
        let state = AppState {
            store: Arc::new(store),
            semaphore: Arc::new(Semaphore::new(1)),
            render_timeout_ms: 1000,
            png_compression: crate::render::Compression::default(),
            // テストでは WebP を有効化（既定 disable とは別に、既存挙動を維持）。
            webp: crate::render::WebpPolicy {
                enabled: true,
                max_area: fulgur_chart::raster_direct::MAX_WEBP_AREA_PIXELS,
            },
            shortlink_cache_control: axum::http::HeaderValue::from_static("public, max-age=86400"),
        };
        axum::Router::new()
            .route("/mcp", post(mcp_handler))
            .with_state(state)
    }

    async fn post_mcp(
        app: axum::Router,
        body: &str,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/mcp")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_owned()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[test]
    fn jsonrpc_error_serializes_null_id() {
        let resp = JsonRpcResponse::error(None, -32700, "Parse error".into());
        let json = serde_json::to_value(&resp).unwrap();
        // id: null は省略ではなく null として出力されること（JSON-RPC 2.0 仕様）
        assert_eq!(json["id"], serde_json::Value::Null);
        assert_eq!(json["error"]["code"], -32700);
    }

    #[test]
    fn extract_valid_id_accepts_string() {
        let mut map = serde_json::Map::new();
        map.insert("id".into(), serde_json::Value::String("abc".into()));
        assert_eq!(
            extract_valid_id(&map),
            Some(serde_json::Value::String("abc".into()))
        );
    }

    #[test]
    fn extract_valid_id_accepts_integer() {
        let mut map = serde_json::Map::new();
        map.insert("id".into(), serde_json::json!(42));
        assert_eq!(extract_valid_id(&map), Some(serde_json::json!(42)));
    }

    #[test]
    fn extract_valid_id_rejects_float() {
        let mut map = serde_json::Map::new();
        map.insert("id".into(), serde_json::json!(1.5));
        assert_eq!(extract_valid_id(&map), None);
    }

    #[test]
    fn extract_valid_id_rejects_null() {
        let mut map = serde_json::Map::new();
        map.insert("id".into(), serde_json::Value::Null);
        assert_eq!(extract_valid_id(&map), None);
    }

    #[test]
    fn extract_valid_id_absent_returns_none() {
        let map = serde_json::Map::new();
        assert_eq!(extract_valid_id(&map), None);
    }

    #[tokio::test]
    async fn malformed_json_returns_parse_error() {
        let (status, json) = post_mcp(test_app().await, "not json").await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(json["error"]["code"], -32700);
        // id は null として出力されること
        assert_eq!(json["id"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn notification_returns_202() {
        let body = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let (status, _) = post_mcp(test_app().await, body).await;
        assert_eq!(status, axum::http::StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn invalid_jsonrpc_echoes_valid_id() {
        // jsonrpc が 2.0 以外でも valid な id があれば echo back する
        let body = r#"{"jsonrpc":"1.0","id":99,"method":"tools/list"}"#;
        let (status, json) = post_mcp(test_app().await, body).await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
        assert_eq!(json["id"], 99);
        assert_eq!(json["error"]["code"], -32600);
    }

    #[tokio::test]
    async fn notification_batch_returns_202() {
        let body = r#"[{"jsonrpc":"2.0","method":"notifications/initialized"}]"#;
        let (status, _) = post_mcp(test_app().await, body).await;
        assert_eq!(status, axum::http::StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn empty_batch_returns_400() {
        let (status, _) = post_mcp(test_app().await, "[]").await;
        assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    }
}
