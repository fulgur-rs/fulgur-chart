use crate::{
    render::{self, OutputFormat},
    state::AppState,
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Map, Value, json};

// JSON-RPC 2.0 レスポンス
#[derive(Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    // Value で受け取り object / array（batch）を分岐する。
    // Map<String, Value> では MCP 2025-03-26 で必須の JSON-RPC batch 配列を拒否してしまう。
    Json(raw): Json<Value>,
) -> Response {
    match raw {
        Value::Array(batch) => {
            // MCP 2025-03-26 Streamable HTTP: 全件 notification の batch → 202 Accepted。
            // request を含む batch は未サポート（experimental）。
            let all_notifications = batch
                .iter()
                .all(|item| item.as_object().is_some_and(|obj| !obj.contains_key("id")));
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

async fn handle_single(raw: Map<String, Value>, state: AppState) -> Response {
    // jsonrpc と method を先に検証してから notification / request を判定する。
    // malformed な object（{"foo":"bar"} 等）が notification として 202 になるのを防ぐ。
    let jsonrpc = raw.get("jsonrpc").and_then(|v| v.as_str()).unwrap_or("");
    if jsonrpc != "2.0" {
        return (
            StatusCode::BAD_REQUEST,
            Json(JsonRpcResponse::error(
                None,
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
                None,
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

    // MCP 2025-03-26: request id は string または integer のみ（null/object/array/bool は不正）。
    let id = match raw.get("id") {
        Some(v @ Value::String(_)) | Some(v @ Value::Number(_)) => Some(v.clone()),
        _ => {
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

    let result = tokio::time::timeout(
        std::time::Duration::from_millis(state.render_timeout_ms),
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let spec = render::parse_and_validate(&json_str, "chartjs", false)?;
            render::render(&spec, format, 1.0)
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
                render::RenderError::Render(_) => -32603, // Internal error
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
